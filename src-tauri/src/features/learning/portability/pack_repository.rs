//! Managed file and database lifecycle for portable Learning Studio packs.
//! This file is the orchestrator; what each pack section does lives in its
//! row in `entries`, what stands in an import's way in `conflicts`, what is
//! recorded in `records`, and where packs go in `files`.
mod canvas_entries;
mod conflicts;
mod entries;
mod evidence_entries;
mod evidence_tables;
mod export;
mod export_all;
mod files;
mod guards;
mod importers;
mod program_entry;
mod provenance;
mod records;
mod remap;
mod replacement;
mod source_entries;
mod validation;

use crate::features::learning::{
    dto::LearningProgramDto,
    pack::{decode_learning_pack, encode_learning_pack, DecodedLearningPack},
    persistence::{db, hash, now},
    portability_dto::*,
};
use entries::ImportScope;
pub use files::DestinationPicker;
use files::{chosen_file, managed_path, next_free_path, read_untrusted};
pub(crate) use files::{pack_file_name, PACK_EXTENSION};
use lattice_core::shared::error::{AppError, Result};
use records::{pack_operation, replay, PendingPreview};
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};
use std::{collections::HashMap, path::Path};
use validation::validate_program;

fn invalid(s: impl Into<String>) -> AppError {
    AppError::InvalidInput(s.into())
}
fn io_error(error: impl std::fmt::Display) -> AppError {
    AppError::Io {
        message: error.to_string(),
        kind: "learning_pack".into(),
    }
}
fn json<T: Serialize>(v: &T) -> Result<Vec<u8>> {
    serde_json::to_vec(v).map_err(|e| AppError::Serialization(e.to_string()))
}
fn json_string<T: Serialize>(v: &T) -> Result<String> {
    serde_json::to_string(v).map_err(|e| AppError::Serialization(e.to_string()))
}
fn parse<T: for<'de> Deserialize<'de>>(v: &[u8]) -> Result<T> {
    serde_json::from_slice(v).map_err(|e| AppError::InvalidData(e.to_string()))
}
fn validate_uuid(id: &str, label: &str) -> Result<()> {
    uuid::Uuid::parse_str(id)
        .map(|_| ())
        .map_err(|_| invalid(format!("Invalid {label} ID.")))
}
fn value_str<'a>(v: &'a serde_json::Value, key: &str) -> Result<&'a str> {
    v.get(key)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| invalid(format!("Pack record has an invalid {key}.")))
}
fn value_i64(v: &serde_json::Value, key: &str) -> Result<i64> {
    v.get(key)
        .and_then(serde_json::Value::as_i64)
        .ok_or_else(|| invalid(format!("Pack record has an invalid {key}.")))
}
fn value_option_str(v: &serde_json::Value, key: &str) -> Result<Option<String>> {
    match v.get(key) {
        Some(serde_json::Value::Null) | None => Ok(None),
        Some(value) => value
            .as_str()
            .map(|s| Some(s.to_owned()))
            .ok_or_else(|| invalid(format!("Pack record has an invalid {key}."))),
    }
}

/// Where `export_to_path` writes. It is resolved only after the replay and
/// the guard, so a retried request or a course that cannot go never opens a
/// dialog.
enum Destination<'a> {
    /// The app's own pack folder.
    Managed,
    /// A file the user picks, offered under the course's title.
    Picked(&'a dyn DestinationPicker),
    /// A free name in a folder already chosen.
    Folder(&'a Path),
}

#[derive(Clone)]
pub struct LearningPackRepository {
    pool: SqlitePool,
    /// App-owned directory where exported packs are written and replacement
    /// backups are kept. The container is deliberately not threaded through
    /// the repository: the feature owns its storage location and the DI
    /// layer supplies it once at construction.
    managed_dir: std::path::PathBuf,
    /// Packs the user saves elsewhere must not land inside this directory.
    app_data_dir: std::path::PathBuf,
}
impl LearningPackRepository {
    pub fn new(
        pool: SqlitePool,
        managed_dir: std::path::PathBuf,
        app_data_dir: std::path::PathBuf,
    ) -> Self {
        Self {
            pool,
            managed_dir,
            app_data_dir,
        }
    }

    pub async fn workspace(&self, program_id: &str) -> Result<LearningPortabilityWorkspaceDto> {
        records::workspace(&self.pool, program_id).await
    }

    /// Into the app's own pack folder, named after the course.
    pub async fn export(
        &self,
        req: &ExportLearningPackRequestDto,
    ) -> Result<LearningPortabilityWorkspaceDto> {
        self.export_to_path(req, Destination::Managed).await?;
        self.workspace(&req.program_id).await
    }

    /// To a file the user picks in `picker`.
    pub async fn save_as(
        &self,
        req: &ExportLearningPackRequestDto,
        picker: &dyn DestinationPicker,
    ) -> Result<SaveLearningPackAsResultDto> {
        Ok(
            match self
                .export_to_path(req, Destination::Picked(picker))
                .await?
            {
                Some(export) => SaveLearningPackAsResultDto::Saved {
                    export: Box::new(export),
                },
                None => SaveLearningPackAsResultDto::Cancelled,
            },
        )
    }

    /// The one way a course's pack is written, and the only place an export
    /// consults the replay ledger: an identical earlier request answers with
    /// what it wrote, while that file is still there. Otherwise the course is
    /// checked, named after its title, given a destination and written,
    /// replacing a file there atomically. `None` when the user dismissed the
    /// dialog.
    async fn export_to_path(
        &self,
        req: &ExportLearningPackRequestDto,
        destination: Destination<'_>,
    ) -> Result<Option<LearningPackExportDto>> {
        validate_uuid(&req.operation_id, "operation")?;
        validate_uuid(&req.program_id, "program")?;
        let payload = hash(req)?;
        if let Some(done) = records::replayed_export(&self.pool, req, &payload).await? {
            return Ok(Some(done));
        }
        let mut connection = self.pool.acquire().await.map_err(db)?;
        let title = guards::require_exportable(&mut connection, &req.program_id)
            .await?
            .summary
            .title;
        drop(connection);
        let path = match destination {
            Destination::Managed => managed_path(&self.managed_dir, &title).await?,
            Destination::Folder(dir) => next_free_path(dir, &pack_file_name(&title)).await?,
            Destination::Picked(picker) => match picker.pick_file(&pack_file_name(&title)).await? {
                Some(chosen) => chosen_file(&self.app_data_dir, &chosen).await?,
                None => return Ok(None),
            },
        };
        let snapshot = export::snapshot(&self.pool, &req.program_id, &req.options).await?;
        let bytes = encode_learning_pack(snapshot)?;
        files::atomic_write(&path, &bytes).await?;
        let manifest = decode_learning_pack(&bytes)?.manifest;
        let destination = path.to_string_lossy().into_owned();
        records::record_export(&self.pool, req, &payload, destination, manifest)
            .await
            .map(Some)
    }

    /// Decode, refuse another build's pack, then report every conflict the
    /// chosen policy would meet.
    pub async fn preview(
        &self,
        req: &PreviewLearningPackImportRequestDto,
    ) -> Result<LearningPortabilityWorkspaceDto> {
        validate_uuid(&req.operation_id, "operation")?;
        validate_uuid(&req.preview_id, "preview")?;
        let payload = hash(req)?;
        if replay(&self.pool, &req.operation_id, "preview_import", &payload)
            .await?
            .is_some()
        {
            return self.workspace_for_preview(&req.preview_id).await;
        }
        let decoded = decode_learning_pack(&read_untrusted(Path::new(&req.source_path)).await?)?;
        let preview = match conflicts::schema_mismatch(&self.pool, &decoded).await? {
            Some(blocked) => blocked,
            None => {
                let program = program_entry::load_program(&decoded)?;
                validate_program(&program)?;
                let checked = entries::check(&decoded, &program)?;
                conflicts::scan(&self.pool, &program, req.conflict_policy, checked).await?
            }
        };
        let program_id =
            records::record_preview(&self.pool, req, &payload, &decoded, preview).await?;
        self.workspace(&program_id).await
    }

    pub async fn cancel(
        &self,
        req: &CancelLearningPackImportPreviewRequestDto,
    ) -> Result<LearningPortabilityWorkspaceDto> {
        validate_uuid(&req.operation_id, "operation")?;
        validate_uuid(&req.preview_id, "preview")?;
        let payload = hash(req)?;
        if replay(&self.pool, &req.operation_id, "cancel_import", &payload)
            .await?
            .is_some()
        {
            return self.workspace_for_preview(&req.preview_id).await;
        }
        let now = now();
        let mut tx = self.pool.begin().await.map_err(db)?;
        let row =
            sqlx::query("SELECT program_id,status FROM learning_pack_import_previews WHERE id=?")
                .bind(&req.preview_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?
                .ok_or_else(|| AppError::NotFound("Pack preview not found".into()))?;
        if row.get::<String, _>("status") != "pending" {
            return Err(invalid("Only a pending pack preview can be cancelled."));
        }
        let program_id: String = row.get("program_id");
        sqlx::query("UPDATE learning_pack_import_previews SET status='cancelled',decided_at=? WHERE id=? AND status='pending'").bind(now).bind(&req.preview_id).execute(&mut *tx).await.map_err(db)?;
        pack_operation(&req.operation_id, "cancel_import", &payload)
            .record(&mut *tx, &req.preview_id)
            .await?;
        tx.commit().await.map_err(db)?;
        self.workspace(&program_id).await
    }

    /// Re-read and re-check the previewed pack, then write it in one
    /// transaction: a replacement is backed up and removed first, and the
    /// notebook links of the course it replaces are carried over.
    pub async fn apply(
        &self,
        req: &ApplyLearningPackImportRequestDto,
    ) -> Result<LearningPortabilityWorkspaceDto> {
        validate_uuid(&req.operation_id, "operation")?;
        validate_uuid(&req.preview_id, "preview")?;
        let payload = hash(req)?;
        if replay(&self.pool, &req.operation_id, "apply_import", &payload)
            .await?
            .is_some()
        {
            return self.workspace_for_preview(&req.preview_id).await;
        }
        let preview = records::pending_preview(&self.pool, &req.preview_id).await?;
        // Re-read current bytes; a preview never authorizes changed data.
        let decoded =
            decode_learning_pack(&read_untrusted(Path::new(&preview.source_path)).await?)?;
        if decoded.manifest.root_sha256 != preview.root_sha256
            || decoded.manifest.root_sha256 != req.expected_root_sha256
        {
            return Err(invalid(
                "The pack changed after preview. Preview it again before applying.",
            ));
        }
        conflicts::require_same_schema(&self.pool, &decoded).await?;
        let original = program_entry::load_program(&decoded)?;
        validate_program(&original)?;
        entries::check(&decoded, &original)?;
        let (program, ids) = self.target_program(&decoded, &original, &preview).await?;
        let replaces = preview.policy.on_existing() == ExistingAction::Replace;
        // Lock before capturing the backup, not merely before deleting rows.
        // Background saves either precede that snapshot or wait for completion.
        let (mut tx, backup) = replacement::begin_apply(
            &self.pool,
            &self.managed_dir,
            &req.preview_id,
            &preview.target_id,
            replaces,
        )
        .await?;
        let mut note_links = None;
        if replaces {
            note_links = Some(replacement::snapshot_note_links(&mut tx, &preview.target_id).await?);
            sqlx::query("DELETE FROM study_decks WHERE id=(SELECT deck_id FROM learning_memory WHERE program_id=? AND deck_id IS NOT NULL)")
                .bind(&preview.target_id).execute(&mut *tx).await.map_err(db)?;
            sqlx::query("DELETE FROM learning_programs WHERE id=?")
                .bind(&preview.target_id)
                .execute(&mut *tx)
                .await
                .map_err(db)?;
        }
        let scope = ImportScope {
            pack: &decoded,
            original: &original,
            program: &program,
            ids: &ids,
            policy: preview.policy,
        };
        importers::run(&mut tx, &scope).await?;
        if let Some(saved) = note_links {
            // After the pack's memory record, so the notebook link is restored onto it.
            replacement::restore_note_links(&mut tx, &program.summary.id, saved).await?;
        }
        records::record_import(
            &mut tx,
            req,
            &payload,
            &program.summary.id,
            backup.as_ref(),
            &preview.changes,
        )
        .await?;
        tx.commit().await.map_err(db)?;
        self.workspace(&program.summary.id).await
    }

    /// The program as it will be written here, with the identifier map that
    /// gets it there: a copy gets fresh identifiers everywhere; any other
    /// policy imports the course under the preview's target ID.
    async fn target_program(
        &self,
        pack: &DecodedLearningPack,
        original: &LearningProgramDto,
        preview: &PendingPreview,
    ) -> Result<(LearningProgramDto, HashMap<String, String>)> {
        let existing: Option<i64> =
            sqlx::query_scalar("SELECT 1 FROM learning_programs WHERE id=?")
                .bind(&preview.target_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(db)?;
        if preview.policy.on_existing() == ExistingAction::Block && existing.is_some() {
            return Err(invalid(
                "Merge-safe import is blocked because the incoming program ID already exists.",
            ));
        }
        let mut program = original.clone();
        let mut ids = HashMap::new();
        if preview.policy.remaps_ids() {
            remap::map_id(&mut ids, &program.summary.id, preview.target_id.clone());
            entries::map_ids(pack, original, &mut ids)?;
            remap::rewrite_program(&mut program, &ids);
        } else {
            program.summary.id = preview.target_id.clone();
        }
        Ok((program, ids))
    }

    async fn workspace_for_preview(&self, id: &str) -> Result<LearningPortabilityWorkspaceDto> {
        let program_id = records::preview_program(&self.pool, id).await?;
        self.workspace(&program_id).await
    }
}
