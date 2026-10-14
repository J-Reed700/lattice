//! Every course, one pack each, into one folder: a backup, or a move of a
//! whole library. One course failing does not stop the others.
use super::{
    destinations::{next_free_path, pack_file_name},
    hash, pack_operation, validate_uuid, LearningPackRepository,
};
use crate::features::learning::{
    portability_dto::{
        ExportAllLearningPacksRequestDto, ExportAllLearningPacksResultDto,
        ExportLearningPackRequestDto, ExportedLearningPackDto, FailedLearningPackExportDto,
    },
    repository::LearningRepository,
};
use crate::shared::error::{AppError, Result};
use sha2::{Digest, Sha256};
use std::path::Path;

const KIND: &str = "export_all";

/// A course's operation ID within a batch: the same batch, folder and course
/// always give the same ID, so a retry into the same folder replays the
/// exports already written there instead of writing `(2)` copies, and a
/// retry into another folder writes fresh packs there. Name-based like
/// UUIDv5, over SHA-256.
fn course_operation_id(batch: &uuid::Uuid, dir: &Path, program_id: &str) -> String {
    let folder = dir.as_os_str().as_encoded_bytes();
    let mut hasher = Sha256::new();
    hasher.update(batch.as_bytes());
    hasher.update((folder.len() as u64).to_le_bytes());
    hasher.update(folder);
    hasher.update(program_id.as_bytes());
    let digest = hasher.finalize();
    let bytes: [u8; 16] = digest.first_chunk().copied().unwrap_or_default();
    uuid::Builder::from_custom_bytes(bytes)
        .into_uuid()
        .to_string()
}

/// The user-facing reason, without the generic error-category wrapper.
fn reason(error: &AppError) -> String {
    match error {
        AppError::InvalidInput(message) | AppError::NotFound(message) => message.clone(),
        other => other.to_string(),
    }
}

impl LearningPackRepository {
    /// The result of an identical earlier batch, if one completed.
    pub async fn replayed_export_all(
        &self,
        req: &ExportAllLearningPacksRequestDto,
    ) -> Result<Option<ExportAllLearningPacksResultDto>> {
        validate_uuid(&req.operation_id, "operation")?;
        pack_operation(&req.operation_id, KIND, &hash(req)?)
            .replay(&self.pool)
            .await
    }

    pub async fn export_all(
        &self,
        req: &ExportAllLearningPacksRequestDto,
        chosen_dir: &Path,
    ) -> Result<ExportAllLearningPacksResultDto> {
        if let Some(done) = self.replayed_export_all(req).await? {
            return Ok(done);
        }
        let batch = uuid::Uuid::parse_str(&req.operation_id)
            .map_err(|_| AppError::InvalidInput("Invalid operation ID.".into()))?;
        let dir = super::destinations::chosen_folder(&self.app_data_dir, chosen_dir).await?;
        let mut result = ExportAllLearningPacksResultDto {
            cancelled: false,
            destination_dir: Some(dir.to_string_lossy().into_owned()),
            exported: Vec::new(),
            failed: Vec::new(),
        };
        for course in LearningRepository::new(self.pool.clone()).list().await? {
            let request = ExportLearningPackRequestDto {
                operation_id: course_operation_id(&batch, &dir, &course.id),
                program_id: course.id.clone(),
                file_name: pack_file_name(&course.title),
                include_evidence: req.include_evidence,
                include_practical_artifacts: req.include_practical_artifacts,
                include_source_bodies: req.include_source_bodies,
                source_body_redistribution_confirmed: req.source_body_redistribution_confirmed,
            };
            match self.export_course_into(&request, &dir).await {
                Ok(path) => result.exported.push(ExportedLearningPackDto {
                    program_id: course.id,
                    title: course.title,
                    path,
                }),
                Err(error) => result.failed.push(FailedLearningPackExportDto {
                    program_id: course.id,
                    title: course.title,
                    error: reason(&error),
                }),
            }
        }
        pack_operation(&req.operation_id, KIND, &hash(req)?)
            .record(&self.pool, &result)
            .await?;
        Ok(result)
    }

    async fn export_course_into(
        &self,
        req: &ExportLearningPackRequestDto,
        dir: &Path,
    ) -> Result<String> {
        if let Some(done) = self.replayed_export(req).await? {
            return Ok(done.destination_path);
        }
        let path = next_free_path(dir, &req.file_name).await?;
        Ok(self.export_to_path(req, &path).await?.destination_path)
    }
}
